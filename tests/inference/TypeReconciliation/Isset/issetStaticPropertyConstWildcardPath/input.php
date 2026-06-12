<?php
final class TaintTypes {
    public const INPUT_HTML = 'html';
    public const INPUT_SQL = 'sql';
}

class ParamStore {
    /** @var array<array-key, string>|null */
    public ?array $sinks = null;
}

class Handler {
    /** @var non-empty-array<string, non-empty-list<list<TaintTypes::*>>>|null */
    private static ?array $taint_sink_map = null;

    public static function assign(ParamStore $p, string $key, int $offset): void {
        self::$taint_sink_map = ['a' => [[TaintTypes::INPUT_HTML]]];
        if (isset(self::$taint_sink_map[$key][$offset])) {
            $p->sinks = self::$taint_sink_map[$key][$offset];
        }
    }
}
