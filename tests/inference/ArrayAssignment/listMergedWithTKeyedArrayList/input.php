<?php
/** @param list<int> $arr */
function takesAnotherList(array $arr) : void {}

/** @param list<int> $arr */
function takesList(array $arr) : void {
    if (rand(0, 1)) {
        $arr = [1, 2, 3];
    }

    takesAnotherList($arr);
}
