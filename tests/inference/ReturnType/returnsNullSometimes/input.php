<?php
/** @return null */
function f() {
    if (rand(0, 1)) {
        return null;
    }
    throw new RuntimeException;
}
