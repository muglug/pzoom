<?php
/**
 * @param mixed $maybe
 */
function matchesTypes($maybe) : void {
    $t = gettype($maybe);
    if ($t === "bool") {}
}
