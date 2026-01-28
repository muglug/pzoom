<?php
/** @param mixed $clips */
function foo($clips, bool $found, int $id) : void {
    if ($found === false) {
        $clips = [];
    }

    $i = array_search($id, $clips);

    if ($i !== false) {
        unset($clips[$i]);
    }
}