<?php
/**
 * @param mixed $_p
 * @psalm-assert-if-true Exception $_p
 * @psalm-assert-if-false Error $_p
 * @psalm-assert Throwable $_p
 */
function f($_p): bool {
    return true;
}

$q = null;
if (rand(0, 1) && f($q)) {}
if (!f($q)) {}
