import Foundation
import WebKit

/// Converte HTML in PDF con WebKit. Le webview di Tauri non supportano
/// `window.print()`, quindi la stampa va fatta qui: WebKit impagina l'HTML
/// esattamente come lo mostra a schermo, comprese tabelle e interruzioni.
@available(macOS 11.0, *)
private final class PdfMaker: NSObject, WKNavigationDelegate {
    private let webView: WKWebView
    private let destinazione: URL
    private var esito: Int32 = 1
    private var finito = false

    init(destinazione: URL) {
        // A4 a 72 punti per pollice, con margini di due centimetri.
        let cornice = NSRect(x: 0, y: 0, width: 595, height: 842)
        webView = WKWebView(frame: cornice, configuration: WKWebViewConfiguration())
        self.destinazione = destinazione
        super.init()
        webView.navigationDelegate = self
    }

    func render(html: String) -> Int32 {
        webView.loadHTMLString(html, baseURL: nil)

        // Attende il completamento pompando il run loop: bloccarlo con un
        // semaforo impedirebbe a WebKit di finire il caricamento.
        let scadenza = Date().addingTimeInterval(30)
        while !finito && Date() < scadenza {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        return esito
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // Un istante perché i fogli di stile vengano applicati davvero.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
            guard let self else { return }

            let configurazione = WKPDFConfiguration()
            self.webView.createPDF(configuration: configurazione) { risultato in
                switch risultato {
                case .success(let dati):
                    do {
                        try dati.write(to: self.destinazione)
                        self.esito = 0
                    } catch {
                        self.esito = 2
                    }
                case .failure:
                    self.esito = 3
                }
                self.finito = true
            }
        }
    }

    func webView(
        _ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error
    ) {
        esito = 4
        finito = true
    }
}

@_cdecl("brief_html_to_pdf")
public func brief_html_to_pdf(
    _ html: UnsafePointer<CChar>, _ output: UnsafePointer<CChar>
) -> Int32 {
    guard #available(macOS 11.0, *) else { return 5 }

    let contenuto = String(cString: html)
    let destinazione = URL(fileURLWithPath: String(cString: output))

    var esito: Int32 = 1
    if Thread.isMainThread {
        esito = PdfMaker(destinazione: destinazione).render(html: contenuto)
    } else {
        DispatchQueue.main.sync {
            esito = PdfMaker(destinazione: destinazione).render(html: contenuto)
        }
    }
    return esito
}
