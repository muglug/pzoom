<?php
/**
 * @return mixed
 */
function getBar(array $array) {
    if (isset($array['foo']['bar'])) {
        return $array['foo']['baz'];
    }

    return [];
}