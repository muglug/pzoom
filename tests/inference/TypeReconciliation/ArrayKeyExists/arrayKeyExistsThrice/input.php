<?php
function three(array $a): void {
    if (!array_key_exists("a", $a)
        || !array_key_exists("b", $a)
        || !array_key_exists("c", $a)
        || (!is_string($a["a"]) && !is_int($a["a"]))
        || (!is_string($a["b"]) && !is_int($a["b"]))
        || (!is_string($a["c"]) && !is_int($a["c"]))
    ) {
        throw new \Exception();
    }

    echo $a["a"];
    echo $a["b"];
}