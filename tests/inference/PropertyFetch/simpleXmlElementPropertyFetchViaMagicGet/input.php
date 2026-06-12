<?php
function takesXml(SimpleXMLElement $el): void {}

function f(SimpleXMLElement $config_xml): void {
    takesXml($config_xml->projectFiles);
}
