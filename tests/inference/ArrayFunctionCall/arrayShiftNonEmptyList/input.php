<?php
/** @param non-empty-list $arr */
function type_of_array_shift(array $arr) : int {
    if (\is_int($arr[0])) {
        return \array_shift($arr);
    }

    return 0;
}
