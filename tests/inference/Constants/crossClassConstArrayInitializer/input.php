<?php
final class TaintKind2 {
    public const INPUT_HTML = 'html';
    public const INPUT_SQL = 'sql';
}
final class TaintKindGroup2 {
    public const ALL_INPUT = [
        TaintKind2::INPUT_HTML,
        TaintKind2::INPUT_SQL,
    ];
}

function pick(bool $b): array {
    if ($b) {
        $taints = TaintKindGroup2::ALL_INPUT;
    } else {
        $taints = [];
    }
    $taints = array_unique(array_merge($taints, ['x']));
    return $taints;
}
