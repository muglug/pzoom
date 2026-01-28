<?php
$foo = "foo";

function a(): string {
    global $foo;

    return $foo;
}