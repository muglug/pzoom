<?php

abstract class JsonRpcBody
{
    public string $jsonrpc = '2.0';
}

final class Envelope
{
    public function __construct(public ?JsonRpcBody $body = null) {}
}

function takesEnvelope(Envelope $e): void
{
    // $e->body is ?JsonRpcBody. `params` is undefined on JsonRpcBody, so inside
    // the `?? null` the chain `$e->body->params` resolves to null and fetching
    // `->message` on it is a NullPropertyFetch — Psalm reports this even inside
    // isset()/??, then leaves the node untyped so the deeper `->value` member is
    // silent (no MixedPropertyFetch).
    if (($e->body->params->message->value ?? null) === 'ready') {
        echo "ready";
    }
}
