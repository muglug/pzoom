<?php
/** @param list<int> $arr */
function takesAnotherList(array $arr) : void {}

/** @param list<int> $arr */
function takesList(array $arr) : void {
    if ($arr) {
        $arr = [4, 5, 6];
    }

    takesAnotherList($arr);
}
