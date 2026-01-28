<?php
/**
 * @param string[] $list1
 * @param string[] $list2
 */
function contains(array $list1, array $list2, string $element): void
{
    if (in_array($element, $list1, true)) {
    } elseif (in_array($element, $list2, true)) {
    }
}