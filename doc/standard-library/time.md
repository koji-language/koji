# time

The `time` module provides functions to delay execution and measure elapsed time. It is written in Koji (`std/time.kj`), on the clocks of the system.

## Functions

Times are in seconds, as decimals: exact to the nanosecond. A timestamp counts the seconds since **0000-01-01 00:00:00 UTC**, the epoch of Koji (in the proleptic Gregorian calendar, where the year before 1 is 0): `0` is the start of the year 0, and `62167219200` is 1970-01-01 00:00:00, the epoch of Unix: a timestamp of Unix (as files, HTTP and most other systems give) plus `62167219200` is one of Koji.

-   `sleep(seconds: decimal) -> void`  
    Pauses the execution for the specified number of seconds (an `int` is a `decimal` too). Zero or less does not pause. Only the current [task](task.md) pauses: the others go on meanwhile.

-   `now() -> decimal`  
    Returns the current time in seconds since the epoch of Koji. This value reflects the system clock and may change if the system time is adjusted.

-   `monotonic() -> decimal`  
    Returns a monotonically increasing time value, useful for measuring durations. This value is not affected by changes to the system clock.

## Choosing Between `now()` and `monotonic()`

Use `now()` when you need the current timestamp, such as logging the time of an event.  
Use `monotonic()` when you need to measure elapsed time precisely, regardless of changes to the system clock.

## Example

```koji
import time

print('Waiting...')

time.sleep(2)

print('Done!')

immut start = time.monotonic()
// Do something
immut end = time.monotonic()

print(`Elapsed: ${end - start} seconds`)
```
