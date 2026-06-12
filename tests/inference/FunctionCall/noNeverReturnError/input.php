<?php
/**
 * @return string
 */
function foo() {
    if (random_int(0, 1)) {
        exit;
    }

    return "foobar";
}
