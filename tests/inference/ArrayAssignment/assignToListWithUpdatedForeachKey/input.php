<?php
/**
 * @param list<string> $list
 * @return list<string>
 */
function getList(array $list): array {
    foreach ($list as $key => $value) {
        $list[$key + 1] = $value . "!";
    }

    return $list;
}
