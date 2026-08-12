using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: OpenXmlValidator FILE.pptx");
    return 2;
}

using PresentationDocument document = PresentationDocument.Open(args[0], false);
if (document.DocumentType != PresentationDocumentType.Presentation)
{
    Console.Error.WriteLine($"expected Presentation, found {document.DocumentType}");
    return 1;
}

OpenXmlValidator validator = new(FileFormatVersions.Microsoft365);
List<ValidationErrorInfo> errors = validator.Validate(document).ToList();
foreach (ValidationErrorInfo error in errors)
{
    Console.Error.WriteLine(
        $"{error.ErrorType}: {error.Description} " +
        $"part={error.Part?.Uri} path={error.Path?.XPath}"
    );
}

if (errors.Count != 0)
{
    Console.Error.WriteLine($"Open XML validation failed with {errors.Count} error(s)");
    return 1;
}

Console.WriteLine("Open XML SDK validation passed");
return 0;
