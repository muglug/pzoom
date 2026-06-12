<?php
/**
 * @return array<?stdClass>
 */
function getBarWithIsset() {
    if (rand() % 2 > 0) return [new stdClass()];
    if (rand() % 2 > 0) return [null];
    return [2];
}
