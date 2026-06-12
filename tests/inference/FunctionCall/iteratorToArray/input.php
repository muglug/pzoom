<?php
/**
 * @return Generator<stdClass>
 */
function generator(): Generator {
    yield new stdClass;
}

$a = iterator_to_array(generator());
