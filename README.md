# udptun

Multi-socket UDP tunnel

## Usage

    USAGE:
        udptun [FLAGS] [OPTIONS]
    
    FLAGS:
        -h, --help                 Prints help information
        -4                         Exclusively use IPv4
        -6                         Exclusively use IPv6
        -L, --log-data             Print a log line per data packet transferred
        -B, --print-data-buffer    Print the contents of the data buffer for each packet transferred
        -v, --verbose              Print more information
        -V, --version              Prints version information
    
    OPTIONS:
        -b, --bufsize <SIZE>                 Packet buffer size, if smaller than packets sent they will get truncated
                                             [default: 65536]
        -E, --entry <ADDRESS>                Specifies that this is the tunnel entry point; the specified address is the one
                                             clients connect to
        -f, --format <FORMAT>                Set the log line format
        -l, --listen <ADDRESS>               The address/port to use for communication inside the tunnel
        -r, --remote <ADDRESS>               Specifies the address of the other end of the tunnel
            --source-format <ADDRESS-FMT>    Specifies the IP address range for created dummy client sockets
        -T, --target <ADDRESS>               Specifies that this is the end of the tunnel the actual server is at; the
                                             specified address is the one of the actual server to proxy
        -x, --timeout <SECS>                 Time in seconds after the last received packet after which a connection is
                                             determined closed [default: 3600]


## How does it work?

                                          tunneled connection
                                          (can be established
                    .----------.            by either side*)
                   /            \                  |                    .------ Client
                  /              \                 v                   /
     Target server -------------- udptun -T ================ udptun -E -------- Client
     (not neces-  \              / (--target)                (--entry) \
      sarily acces-\            /                                       *------ Client
      sible by the  *----------*                                                  ^
      clients)           ^                                                        |
                         |                                                clients connected
                one connection per                                       to tunnel entrypoint
                client connected to
                 tunnel entrypoint

    *: the side that establishes the connection is the one that does not use the --listen flag,
       remote tunnel address specified by --remote
       note: this is seperate from --target/--entry!