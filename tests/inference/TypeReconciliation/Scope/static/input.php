<?php
function a(): string {
    static $foo = "foo";

    return $foo;
}
