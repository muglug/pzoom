<?php
/** @return never */
function returnsNever() {
    exit();
}

/** @return false */
function foo() : bool {
    if (rand(0, 1)) {
        return false;
    }

    returnsNever();
}
