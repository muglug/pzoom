<?php
/**
 * @param string|int $val
 * @param string|array $text
 * @param array $data
 */
function _renderInput($val, $text, $data) : array {
    if (is_int($val) && isset($text["foo"], $text["bar"])) {
        $radio = $text;
    } else {
        $radio = ["value" => $val, "text" => $text];
    }
    return $radio;
}