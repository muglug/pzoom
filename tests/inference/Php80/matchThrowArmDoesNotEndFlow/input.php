<?php
function f(int $i): string {
    $x = match ($i) {
        1 => "a",
        2 => "b",
        default => throw new UnexpectedValueException("nope"),
    };

    $y = strlen($x);
    return $x . $y;
}
