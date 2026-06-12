<?php
/**
 * @param non-empty-array|null $arg
 * @return void
 */
function foo($arg) {
    if ($arg) {
    }

    if (!$arg) {
    }

    if (bar($arg)) {
    }

    if (!bar($arg)) {
    }
}

/**
 * @param mixed $arg
 * @return non-empty-array|null
 */
function bar($arg) {}
