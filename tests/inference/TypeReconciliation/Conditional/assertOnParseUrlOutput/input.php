<?php
/**
 * @param array<"a"|"b"|"c", mixed> $arr
 */
function uriToPath(array $arr) : string {
    if (!isset($arr["a"]) || $arr["b"] !== "foo") {
        throw new \InvalidArgumentException("bad");
    }

    return (string) $arr["c"];
}