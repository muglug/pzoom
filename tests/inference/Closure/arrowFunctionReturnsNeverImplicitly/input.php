<?php
$bar = ["foo", "bar"];

$bam = array_map(
    fn(string $a) => throw new Exception($a),
    $bar
);
