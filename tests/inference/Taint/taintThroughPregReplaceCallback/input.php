<?php
$a = $_GET["bad"];

$b = preg_replace_callback(
    '/foo/',
    function (array $matches) : string {
        return $matches[1];
    },
    $a
);

echo $b;
