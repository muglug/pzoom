<?php
final class A26 {}
/**
 * @param array<string, non-empty-array<string, A26>> $pa
 * @param array<string, non-empty-array<string, A26>> $pb
 */
function h(array $pa, array $pb): void {
    $acc = [];
    foreach ($pa as $var_id => $possibilities) {
        if (!isset($acc[$var_id])) {
            $acc[$var_id] = $possibilities;
        } else {
            $acc[$var_id] = array_merge($acc[$var_id], $possibilities);
        }
    }
    foreach ($pb as $var_id => $possibilities) {
        if (!isset($acc[$var_id])) {
            $acc[$var_id] = $possibilities;
        } else {
            $acc[$var_id] = array_merge($acc[$var_id], $possibilities);
        }
    }
    return;
}

/** @param array<string, non-empty-array<string, A26>> $expected */
function takesAcc(array $expected): void {}

/**
 * @param array<string, non-empty-array<string, A26>> $pa
 * @param array<string, non-empty-array<string, A26>> $pb
 */
function h2(array $pa, array $pb): void {
    $acc = [];
    foreach ($pa as $var_id => $possibilities) {
        if (!isset($acc[$var_id])) {
            $acc[$var_id] = $possibilities;
        } else {
            $acc[$var_id] = array_merge($acc[$var_id], $possibilities);
        }
    }
    foreach ($pb as $var_id => $possibilities) {
        if (!isset($acc[$var_id])) {
            $acc[$var_id] = $possibilities;
        } else {
            $acc[$var_id] = array_merge($acc[$var_id], $possibilities);
        }
    }
    takesAcc($acc);
}
