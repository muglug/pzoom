<?php
function acceptsStringable(Stringable $_p): void {}
/** @param stringable-object $p */
function f(object $p): void
{
    f($p);
}
