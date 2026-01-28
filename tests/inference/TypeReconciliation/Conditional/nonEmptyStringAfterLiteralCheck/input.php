<?php
/**
 * @param non-empty-string $greeting
 */
function sayHi(string $greeting): void {
    echo $greeting;
}

/** @var string */
$hello = "foo";

if ($hello === "") {
    throw new \Exception("an empty string is not a greeting");
}

sayHi($hello);