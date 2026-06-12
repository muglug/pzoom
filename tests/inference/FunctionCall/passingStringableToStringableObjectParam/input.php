<?php
/** @param stringable-object $_o */
function acceptsStringableObject(object $_o): void {}

function f(Stringable $o): void
{
    acceptsStringableObject($o);
}
