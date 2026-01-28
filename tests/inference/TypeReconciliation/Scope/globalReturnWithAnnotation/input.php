<?php
/**
 * @global string $foo
 */
function a(): string {
    global $foo;

    return $foo;
}