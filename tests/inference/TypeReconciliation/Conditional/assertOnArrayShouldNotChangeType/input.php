<?php
/** @return array|string|false */
function foo(string $a, string $b) {
    $options = getopt($a, [$b]);

    if (isset($options["config"])) {
        $options["c"] = $options["config"];
    }

    if (isset($options["root"])) {
        return $options["root"];
    }

    return false;
}