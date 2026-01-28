<?php
/**
 * @return array{"a1", "a2"}
 */
function getSupportedConsts() {
    return ["a1", "a2"];
}

function foo(mixed $file) : string {
    if (in_array($file, getSupportedConsts(), true)) {
        return $file;
    }

    return "";
}