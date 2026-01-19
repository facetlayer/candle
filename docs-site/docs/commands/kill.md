# kill

Kill running services.

## Syntax

```bash
candle kill [name...]
```

## Description

The `kill` command stops running services.

When called without arguments, it kills all services in the current project directory.

## Arguments

- `name` - Name of the service(s) to kill. If omitted, kills all services in the current project.

## Examples

### Kill a specific service

```bash
candle kill api
```

### Kill multiple services

```bash
candle kill api web worker
```

### Kill all services in current project

```bash
candle kill
```

## See Also

- [kill-all](kill-all) - Kill all services globally
- [start](start) - Start services
- [restart](restart) - Restart services
