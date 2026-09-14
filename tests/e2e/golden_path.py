from pathlib import Path
import re
from playwright.sync_api import sync_playwright


def main() -> None:
    evidence = Path("docs/evidence")
    evidence.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, executable_path=r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.set_default_timeout(3000)
        page.goto("http://127.0.0.1:1420", wait_until="domcontentloaded")
        page.wait_for_timeout(500)
        page.screenshot(path=str(evidence / "secrethub-ui-zh.png"), full_page=True)
        assert page.get_by_text("凭据中心").is_visible()
        page.get_by_role("button", name="English").click()
        assert page.get_by_text("Secret catalog").is_visible()
        assert page.get_by_role("button", name="中文").is_visible()
        page.get_by_role("button", name="中文").click()
        assert page.get_by_text("凭据中心").is_visible()
        page.get_by_role("button", name="添加凭据").first.click()
        dialog = page.get_by_role("dialog")
        dialog.get_by_role("button", name="API KEY").click()
        dialog.get_by_role("button", name=re.compile("OpenAI")).click()
        dialog.get_by_label("名称").fill("Browser Fixture")
        assert dialog.get_by_label("环境变量名").get_attribute("required") is None
        dialog.get_by_label("Secret 值").fill("fixture-only-value")
        dialog.get_by_role("button", name="保存凭据").click()
        assert page.get_by_text("Browser Fixture").is_visible()
        page.get_by_label("导出 OpenAI Main").check()
        page.get_by_role("button", name="导出").click()
        export_dialog = page.get_by_role("dialog")
        export_dialog.get_by_role("button", name="预览 .env").click()
        assert "••••••••" in export_dialog.locator(".preview-box").inner_text()
        page.screenshot(path=str(evidence / "secrethub-ui-en-export.png"), full_page=True)
        export_dialog.locator("button.close-button").click()
        page.get_by_role("button", name="导入").click()
        import_dialog = page.get_by_role("dialog")
        import_dialog.get_by_label("粘贴 .env 或 JSON 内容").fill("IMPORTED_FIXTURE=fixture-only-value")
        import_dialog.get_by_role("button", name="预览字段").click()
        assert "IMPORTED_FIXTURE = [encrypted]" in import_dialog.locator(".preview-box").inner_text()
        browser.close()


if __name__ == "__main__":
    main()
