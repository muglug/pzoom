<?php
/**
 * @param "array"|class-string $type_name
 */
function foo(string $type_name) : bool {
    return $type_name === "array";
}
