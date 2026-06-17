<?php

class TraceNode {
    public ?TraceNode $previous = null;
    public string $label = '';
}

/** @return list<string> */
function getIssueTrace(TraceNode $source): array
{
    $out = [];
    do {
        /** @var TraceNode $source */
        $previous_source = $source->previous;
        if ($previous_source === $source) {
            break;
        }
        array_unshift($out, $source->label);
        $source = $previous_source;
    } while ($previous_source);

    return $out;
}
