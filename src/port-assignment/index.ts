import { createServer } from 'node:http';
import { getDatabase } from '../database/database.ts';
import { UsageError } from '../errors.ts';

const MIN_PORT = 4000;
const MAX_PORT = 65535;

export interface ReservedPort {
  port: number;
  project_dir: string;
  service_name: string | null;
  assigned_at: number;
}

export interface ReservePortOptions {
  projectDir: string;
  serviceName?: string;
}

export interface ReleasePortsOptions {
  projectDir: string;
  serviceName?: string;
}

/**
 * Check if a port is actually available by attempting to bind to it
 */
export async function isPortActuallyAvailable(port: number): Promise<boolean> {
  return new Promise(resolve => {
    const server = createServer();

    server.listen(port, '127.0.0.1', () => {
      server.close(() => {
        resolve(true);
      });
    });

    server.on('error', () => {
      resolve(false);
    });
  });
}

/**
 * Get the next port to try from the tracking table.
 * If no row exists, initialize it with the starting value.
 */
function getAndIncrementNextPort(): number {
  const db = getDatabase();

  const row = db.get('SELECT port FROM next_port_to_try WHERE id = 1') as { port: number } | undefined;

  if (!row) {
    // Initialize with starting value
    db.insert('next_port_to_try', { id: 1, port: MIN_PORT + 1 });
    return MIN_PORT;
  }

  const currentPort = row.port;

  // Calculate next port with wraparound
  let nextPort = currentPort + 1;
  if (nextPort > MAX_PORT) {
    nextPort = MIN_PORT;
  }

  // Update the next port value
  db.run('UPDATE next_port_to_try SET port = ? WHERE id = 1', nextPort);

  return currentPort;
}

/**
 * Check if a port is already reserved in the database
 */
export function isPortReserved(port: number): boolean {
  const db = getDatabase();
  const row = db.get('SELECT port FROM reserved_ports WHERE port = ?', [port]);
  return !!row;
}

/**
 * Get the reserved port for a specific service
 */
export function getReservedPortForService(projectDir: string, serviceName: string): ReservedPort | null {
  const db = getDatabase();
  const row = db.get(
    'SELECT * FROM reserved_ports WHERE project_dir = ? AND service_name = ?',
    [projectDir, serviceName]
  ) as ReservedPort | undefined;
  return row || null;
}

/**
 * Get all reserved ports for a project directory
 */
export function getReservedPortsForProject(projectDir: string): ReservedPort[] {
  const db = getDatabase();
  return db.list(
    'SELECT * FROM reserved_ports WHERE project_dir = ? ORDER BY assigned_at DESC',
    [projectDir]
  ) as ReservedPort[];
}

/**
 * Get all reserved ports
 */
export function getAllReservedPorts(): ReservedPort[] {
  const db = getDatabase();
  return db.list('SELECT * FROM reserved_ports ORDER BY assigned_at DESC') as ReservedPort[];
}

/**
 * Reserve a port in the database
 */
function insertReservedPort(port: number, projectDir: string, serviceName?: string): void {
  const db = getDatabase();
  db.insert('reserved_ports', {
    port,
    project_dir: projectDir,
    service_name: serviceName || null,
  });
}

/**
 * Release a specific port
 */
export function releasePort(port: number): boolean {
  const db = getDatabase();
  const result = db.run('DELETE FROM reserved_ports WHERE port = ?', [port]);
  return result.changes > 0;
}

/**
 * Release all ports for a project directory
 */
export function releasePortsForProject(projectDir: string): number {
  const db = getDatabase();
  const result = db.run('DELETE FROM reserved_ports WHERE project_dir = ?', [projectDir]);
  return result.changes;
}

/**
 * Release the port for a specific service
 */
export function releasePortForService(projectDir: string, serviceName: string): boolean {
  const db = getDatabase();
  const result = db.run(
    'DELETE FROM reserved_ports WHERE project_dir = ? AND service_name = ?',
    [projectDir, serviceName]
  );
  return result.changes > 0;
}

/**
 * Reserve an unused port.
 *
 * This function:
 * 1. Gets the next port from the tracking table (or initializes at MIN_PORT)
 * 2. Checks if the port is already reserved in the database
 * 3. Verifies the port is actually available on the system
 * 4. If not available, keeps trying with incremented values
 * 5. Assigns the port in the database
 * 6. Returns the port number
 *
 * @param options - Configuration options for reserving a port
 * @returns The reserved port number
 */
export async function reservePort(options: ReservePortOptions): Promise<number> {
  const { projectDir, serviceName } = options;

  // If a service name is provided, check if it already has a reserved port
  if (serviceName) {
    const existingPort = getReservedPortForService(projectDir, serviceName);
    if (existingPort) {
      throw new UsageError(
        `Service '${serviceName}' already has a reserved port: ${existingPort.port}`
      );
    }
  }

  const maxAttempts = MAX_PORT - MIN_PORT + 1;
  const maxRetries = 10;

  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    // Get and increment the next port from our tracking table
    const port = getAndIncrementNextPort();

    // Check if port is already reserved in database
    if (isPortReserved(port)) {
      // Port is already reserved, try the next one
      continue;
    }

    // Check if port is actually available on the system
    const actuallyAvailable = await isPortActuallyAvailable(port);
    if (actuallyAvailable) {
      // Try to insert the port with retry logic for race conditions
      for (let retry = 0; retry < maxRetries; retry++) {
        try {
          insertReservedPort(port, projectDir, serviceName);
          return port;
        } catch (error: any) {
          // Check if it's a unique constraint error (port was claimed by another process)
          if (error?.code === 'SQLITE_CONSTRAINT_PRIMARYKEY') {
            // Another process claimed this port, break and try next port
            break;
          }
          // For other errors, rethrow
          throw error;
        }
      }
    }

    // Port wasn't available or reservation failed, loop will try the next one
  }

  throw new Error('No available ports found after checking all ports in range');
}

/**
 * Reset for testing - clears all port reservations
 */
export function resetPortReservationsForTesting(): void {
  const db = getDatabase();
  db.run('DELETE FROM reserved_ports');
  db.run('DELETE FROM next_port_to_try');
}
