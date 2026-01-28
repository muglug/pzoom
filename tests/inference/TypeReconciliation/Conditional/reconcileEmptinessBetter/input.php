<?php
/**
 * @param string|array $valuePath
 */
function combine($valuePath) : void {
    if (!empty($valuePath) && is_array($valuePath)) {

    } elseif (!empty($valuePath)) {
        echo $valuePath;
    }
}