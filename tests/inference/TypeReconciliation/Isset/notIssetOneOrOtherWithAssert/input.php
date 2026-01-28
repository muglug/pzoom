<?php
$foo = [
    "one" => rand(0,1) ? new DateTime : null,
    "two" => rand(0,1) ? new DateTime : null,
    "three" => new DateTime
];

assert(isset($foo["one"]) || isset($foo["two"]));

echo $foo["one"]->format("Y");