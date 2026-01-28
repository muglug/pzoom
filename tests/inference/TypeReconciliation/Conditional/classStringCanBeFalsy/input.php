<?php
/** @param class-string<stdClass>|null $val */
function foo(?string $val) : void {
    if (!$val) {}
    if ($val) {}
}