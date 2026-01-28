<?php
function indexof(string $haystack, string $needle): int
{
    $pos = strpos($haystack, $needle);

    if ($pos === false) {
        return -1;
    }

    return $pos;
}

$a = indexof("arr", "a");
