<?php

class Storage7 {
    /** @var array<string, array<string, string>>|null */
    public ?array $template_extended_params = null;
}

/** @param array<string, array<string, string>> $m */
function takesMap(array $m): int { return count($m); }

function f(Storage7 $s): int
{
    if (!isset($s->template_extended_params['Traversable'])) {
        return 0;
    }

    return takesMap($s->template_extended_params);
}
