<?php
/**
 * @param array<string, array<string, string>> $array
 * @return array<string, string>
 */
function getBar(array $array) : array {
    if (isset($array['foo']['bar'])) {
        return $array['foo'];
    }

    return [];
}