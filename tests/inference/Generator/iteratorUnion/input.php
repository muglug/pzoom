<?php
/** @return Iterator|IteratorAggregate */
function getIteratorOrAggregate() {
    yield 2;
}
echo json_encode(iterator_to_array(getIteratorOrAggregate()));
