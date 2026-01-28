<?php
namespace Bar;

/**
 * @psalm-assert-if-true string $a
 * @param mixed $a
 */
function my_is_string($a) : bool
{
    return is_string($a);
}

if (my_is_string($_SESSION["abc"])) {
    $i = substr($_SESSION["abc"], 1, 2);
}
