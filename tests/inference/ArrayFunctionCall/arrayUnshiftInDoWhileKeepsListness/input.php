<?php
class TraceNode {
    public ?TraceNode $previous = null;
    public string $label = '';
}

/**
 * @return list<array{label: string}>
 */
function trace(TraceNode $source): array {
    $out = [];
    do {
        $previous_source = $source->previous;
        if ($previous_source === $source) {
            break;
        }
        array_unshift($out, [
            'label' => $source->label,
        ]);
        /** @var TraceNode|null $source */
        $source = $previous_source;
        if ($source === null) {
            break;
        }
    } while (true);

    return $out;
}
