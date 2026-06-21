<?php
/**
 * A `$` inside a quoted literal (`'$dollar'`) is part of the type, not the
 * parameter name: the name is `$id`. Before the fix the name extractor stopped
 * at the `$` inside the quotes, so `$id` kept its bare `string` type and the
 * call below was not checked.
 *
 * @param 'literal'|'$dollar' $id
 */
function takesQuotedDollarLiteral(string $id): void {}

takesQuotedDollarLiteral('other');
