export async function readStdinAsJson(): Promise<any> {
  return new Promise((resolve, reject) => {
    let input = '';

    process.stdin.setEncoding('utf8');

    process.stdin.on('data', chunk => {
      input += chunk;
    });

    process.stdin.on('end', () => {
      try {
        const parsed = JSON.parse(input);
        resolve(parsed);
      } catch (error) {
        reject(new Error(`Failed to parse JSON input: ${error}`));
      }
    });

    process.stdin.on('error', error => {
      reject(new Error(`Failed to read stdin: ${error}`));
    });
  });
}
