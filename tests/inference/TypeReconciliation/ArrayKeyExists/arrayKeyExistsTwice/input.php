<?php
function two(array $a): void {
    if (!array_key_exists("a", $a) || !(is_string($a["a"]) || is_int($a["a"])) ||
        !array_key_exists("b", $a) || !(is_string($a["b"]) || is_int($a["b"]))
    ) {
        throw new \Exception();
    }

    echo $a["a"];
    echo $a["b"];
}