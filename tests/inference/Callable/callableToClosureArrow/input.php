<?php
/**
 * @return callable
 */
function foo() {
    return fn(string $a): string => $a . "blah";
}
