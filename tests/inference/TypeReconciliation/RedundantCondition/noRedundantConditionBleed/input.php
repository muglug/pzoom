<?php
$foo = getopt("i");
$i = $foo["i"];

/** @psalm-suppress TypeDoesNotContainNull */
if ($i === null) {
    exit;
}

if ($i !== array() && $i !== "" && $i !== "0") {}
