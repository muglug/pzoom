<?php
function takesXml(SimpleXMLElement $el): void {}

function f(SimpleXMLElement $config_xml): void {
    if (isset($config_xml->projectFiles)) {
        takesXml($config_xml->projectFiles);
    }
}
