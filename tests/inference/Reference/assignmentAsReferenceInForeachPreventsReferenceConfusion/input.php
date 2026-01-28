<?php
$arr = [1, 2, 3];
foreach ($arr as &$i) {
    ++$i;
}
foreach ($arr as &$i) {
    ++$i;
}
