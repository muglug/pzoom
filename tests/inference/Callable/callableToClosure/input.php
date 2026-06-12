<?php
/**
 * @return callable
 */
function foo() {
    return function(string $a): string {
        return $a . "blah";
    };
}
