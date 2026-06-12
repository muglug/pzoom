<?php
$dummy = ["test" => 123];

/** @var array{test: ?int} */
$a = ["test" => null];

if ($a["test"] === null) {
    $a = $dummy;
}
$var = $a["test"];
