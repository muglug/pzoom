<?php
$foo = [];
do {
    if (isset($foo["bar"])) {}
    $foo["bar"] = "bat";
} while (rand(0, 1));
