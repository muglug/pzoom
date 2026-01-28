<?php
/** @param non-empty-list<int> $arr */
function foo(array $arr) : void {
    while (array_shift($arr)) {
        if ($arr && $arr[0] === "a") {}

        if (rand(0, 1)) {
            $arr = array_merge($arr, ["a"]);
        }

        echo "here";
    }
}
