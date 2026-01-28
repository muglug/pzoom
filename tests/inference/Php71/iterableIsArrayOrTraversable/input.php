<?php
function castToArray(iterable $arr): array {
    if ($arr instanceof \Traversable) {
        return iterator_to_array($arr, false);
    }

    return $arr;
}

function castToArray2(iterable $arr): array {
    if (is_array($arr)) {
        return $arr;
    }

    return iterator_to_array($arr, false);
}
