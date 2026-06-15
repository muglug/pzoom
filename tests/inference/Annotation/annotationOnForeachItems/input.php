<?php
function foo(array $arr) : void {
    $item = null;

    /** @var string $item */
    foreach ($arr as $item) {}

    if (is_null($item)) {}
}

function bar(array $arr) : void {
    $item = null;

    /** @var string $item */
    foreach ($arr as $item => $_) {}

    if (is_null($item)) {}
}

function bat(array $arr) : void {
    $item = null;

    /**
     * @var string $item
     */
    foreach ($arr as list($item)) {}

    if (is_null($item)) {}
}

function baz(array $arr) : void {
    $item = null;

    /**
     * @var string $item
     */
    foreach ($arr as list($item => $_)) {}

    if (is_null($item)) {}
}
