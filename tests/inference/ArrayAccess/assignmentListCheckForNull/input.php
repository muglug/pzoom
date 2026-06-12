<?php
/**
 * @return array{0: int, 1:string}|null
 */
function bar(int $i) {
    if ( $i < 0)
        return [$i, "hello"];
    else
        return null;
}

/** @psalm-suppress PossiblyNullArrayAccess */
[1 => $foo] = bar(0);

if ($foo !== null) {}
