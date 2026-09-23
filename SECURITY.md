# Security

The dashboard/admin control surface and watchdog agents use different bearer
credentials.

Both credentials are generated during bootstrap and excluded from Git.

The remote API cannot specify executable paths, shell commands, command-line
arguments, or arbitrary process IDs.

A watchdog may terminate only the child process that the watchdog itself
launched from its local configuration.

The system does not expose arbitrary remote shell execution.

Fail-closed behavior is local to the node. Loss of contact with the control
plane can therefore contain the supervised workload without waiting for a
remote instruction.

The default HTTP listener binds to loopback.
