<?php
/**
 * @param arraylike-object<int, string>|array<int, string> $arr
 */
function test($arr): string {
    return $arr[0];
}

test(["a", "b"]);
test(new ArrayObject(["a", "b"]));
