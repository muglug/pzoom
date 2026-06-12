<?php
/**
 * @param array{a?: string, b: string} $arr
 */
function createFromString(array $arr): void
{
    if (empty($arr["a"])) {
        return;
    }

    echo $arr["a"];
}
