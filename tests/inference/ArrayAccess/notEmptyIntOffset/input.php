<?php
/**
 * @param  array<string>  $a
 */
function bar (array $a): string {
    if ($a[0]) {
        return $a[0];
    }

    return "blah";
}
