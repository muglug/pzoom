<?php
function contains(array $list1, array $list2, mixed $element): void
{
    if (in_array($element, $list1, true)) {
    } elseif (in_array($element, $list2, true)) {
    }
}