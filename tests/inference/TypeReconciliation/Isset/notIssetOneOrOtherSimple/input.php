<?php
$foo = [
    "one" => rand(0,1) ? new DateTime : null,
    "two" => rand(0,1) ? new DateTime : null,
    "three" => new DateTime
];

if (!(isset($foo["one"]) || isset($foo["two"]))) {
    exit;
}

echo $foo["one"]->format("Y");