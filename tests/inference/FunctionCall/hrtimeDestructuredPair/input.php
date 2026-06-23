<?php
// hrtime() (or hrtime(false)) returns the precise pair array{0: int, 1: int},
// so destructuring both offsets is safe (no PossiblyUndefinedIntArrayOffset).
function elapsed(): int {
    [$seconds, $nseconds] = hrtime();
    return $seconds + $nseconds;
}
