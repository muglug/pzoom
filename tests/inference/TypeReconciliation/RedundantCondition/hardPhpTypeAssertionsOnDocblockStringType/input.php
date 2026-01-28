<?php
/** @param string|null $bar */
function foo($bar): void {
    if (!is_null($bar) && !is_string($bar)) {
        throw new \Exception("bad");
    }

    if ($bar !== null) {}
}