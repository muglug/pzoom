<?php
/** @param bool|null $bar */
function foo($bar): void {
    if (!is_null($bar) && !is_bool($bar)) {
        throw new \Exception("bad");
    }

    if ($bar !== null) {}
}