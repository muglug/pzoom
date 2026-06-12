<?php
/**
 * @param  string[] $arr
 */
function foo(array $arr) : void {
    if (!$arr) {
        return;
    }

    foreach ($arr as $i => $_) {}

    if ($i === "hello") {}
    if ($i !== "hello") {}
    if ($i === 5) {}
    if ($i !== 5) {}
    if (is_string($i)) {}
    if (is_int($i)) {}

    foreach ($arr as $i => $_) {}

    if ($i === "hell") {
        $i = "hellp";
    }

    if ($i === "hel") {}
}
