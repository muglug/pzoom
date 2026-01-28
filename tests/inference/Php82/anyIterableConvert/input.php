<?php
function castToArray(iterable $arr): array {
    return iterator_to_array($arr, false);
}
