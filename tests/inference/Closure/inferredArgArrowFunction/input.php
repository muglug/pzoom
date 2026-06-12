<?php
$bar = ["foo", "bar"];

$bam = array_map(
    fn(string $a) => $a . "blah",
    $bar
);
