<?php
/**
 * @param  iterable<string>|null $foo
 */
function d(?iterable $foo): void {
    if (is_iterable($foo)) {
        foreach ($foo as $f) {}
    }

    if (!is_iterable($foo)) {

    } else {
        foreach ($foo as $f) {}
    }
}