<?php

/**
 * @param list<int> $arr
 */
function takesList(array $arr): void
{
    for ($arr as $i => $v) {
        echo $i . $v;
    }
}
