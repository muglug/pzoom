<?php
/**
 * @param object{g: bool} $o
 */
function f(object $o, bool $b): bool {
    if ($o->g && $b) {
        return $o->g;
    }
    return true;
}
