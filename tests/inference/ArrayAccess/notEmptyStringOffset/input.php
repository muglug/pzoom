<?php
/**
 * @param  array<string>  $a
 */
function bar (array $a): string {
    if ($a["bat"]) {
        return $a["bat"];
    }

    return "blah";
}
